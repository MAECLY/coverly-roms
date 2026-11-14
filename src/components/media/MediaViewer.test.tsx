import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@testing-library/react';
import { useState } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useViewShortcuts } from '@/hooks/useViewShortcuts';
import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend, type Backend } from '@/ipc/backend';
import type {
  CategoryRef,
  ExtraFileView,
  ExtrasView,
  Playable,
} from '@/ipc/types';
import {
  MAX_PIXELS,
  MAX_VIEW_BYTES,
  routeOf,
  viewerQueue,
  zoomStep,
} from '@/lib/viewer';
import { DEFAULT_PREFS, type ViewPrefs } from '@/lib/viewPrefs';
import { usePlayerStore } from '@/stores/playerStore';
import { useViewerStore } from '@/stores/viewerStore';

import { MediaViewer } from './MediaViewer';
import { GAME, listing as trackListing } from './testTracks';

const ARTWORK: CategoryRef = { kind: 'fixed', name: 'artwork' };
const GUIDES: CategoryRef = { kind: 'fixed', name: 'guides' };
const VIDEOS: CategoryRef = { kind: 'fixed', name: 'videos' };
const THEMES: CategoryRef = { kind: 'fixed', name: 'themes' };
const SOUNDTRACK: CategoryRef = { kind: 'fixed', name: 'soundtrack' };
const ROOT: CategoryRef = { kind: 'root' };

function file(
  index: number,
  path: string,
  category: CategoryRef,
  kind: ExtraFileView['kind'],
  playable: Playable = 'inline',
  size = 1000,
): ExtraFileView {
  return {
    index,
    category,
    path,
    name: path.split('/').pop()!,
    kind,
    provenance: 'user',
    format: null,
    tags: [],
    size,
    link: false,
    playable,
  };
}

const FILES: ExtraFileView[] = [
  file(0, 'Soundtrack/01 Opening.flac', SOUNDTRACK, 'audio'),
  file(1, 'Artwork/Box Front.png', ARTWORK, 'image'),
  file(2, 'Artwork/Box Back.png', ARTWORK, 'image'),
  file(3, 'Artwork/Poster.jpg', ARTWORK, 'image'),
  file(4, 'Videos/Trailer.webm', VIDEOS, 'video'),
  file(5, 'Guides/Notes.txt', GUIDES, 'document'),
  file(6, 'Readme.nfo', ROOT, 'document'),
  file(7, 'Themes/Tidewalker.p3t', THEMES, 'other', 'external'),
  file(8, 'Themes/setup.exe', THEMES, 'other', 'reveal-only'),
  file(9, 'Videos/Huge.mov', VIDEOS, 'video', 'inline', MAX_VIEW_BYTES + 1),
  file(10, 'Soundtrack/02 Boss.flac', SOUNDTRACK, 'audio'),
  file(11, 'Soundtrack/03 Boss.spc', SOUNDTRACK, 'audio', 'external'),
];

function view(files = FILES, rev = 4): ExtrasView {
  return { ...trackListing(files, rev, 7), files };
}

function fakeBackend(over: Partial<Backend> = {}) {
  const b = {
    extrasMedia: vi.fn(
      async (_g: number, _e: number, rev: number, index: number) => ({
        url: `blob:media-${rev}-${index}`,
        mime: 'image/png',
        kind: index === 4 ? ('video' as const) : ('image' as const),
      }),
    ),
    extrasReadText: vi.fn(
      async (_g: number, _e: number, _r: number, i: number) =>
        i === 6
          ? {
              text: "╔═╗ TIDEWALKER\n<script>alert('nfo')</script>\n",
              truncated: false,
              encoding: 'cp437' as const,
            }
          : {
              text: 'Notas del juego.\n',
              truncated: true,
              encoding: 'utf-8' as const,
            },
    ),
    extrasReveal: vi.fn(async () => undefined),
    extrasOpenExternal: vi.fn(async () => undefined),
    ...over,
  };
  setBackend(b as unknown as Backend);
  return b;
}

function Shell({
  files = FILES,
  spy,
}: {
  files?: ExtraFileView[];
  spy?: (patch: Partial<ViewPrefs>) => void;
}) {
  const [prefs, setPrefs] = useState<ViewPrefs>({
    ...DEFAULT_PREFS,
    view: 'covers',
    coverSize: 'm',
  });
  useViewShortcuts({
    prefs,
    setPrefs: (p) => {
      spy?.(p);
      setPrefs((x) => ({ ...x, ...p }));
    },
  });
  return (
    <>
      <span data-testid="cover">{prefs.coverSize}</span>
      <ul>
        {files.map((f) => (
          <li key={f.index} data-file={f.index}>
            <button type="button" data-extras-primary>
              {f.name}
            </button>
          </li>
        ))}
      </ul>
      <MediaViewer generation={GAME.generation} listing={view(files)} />
    </>
  );
}

function show(props: Parameters<typeof Shell>[0] = {}) {
  return render(
    <I18nProvider locale="es">
      <Shell {...props} />
    </I18nProvider>,
  );
}

function openAt(path: string) {
  act(() =>
    useViewerStore
      .getState()
      .show({ generation: GAME.generation, entryId: 7, path }),
  );
}

function reduceMotion(on: boolean) {
  vi.stubGlobal(
    'matchMedia',
    vi.fn((q: string) => ({
      matches: on && q.includes('reduce'),
      media: q,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
    })),
  );
}

const dialog = () => screen.getByRole('dialog');

describe('viewer helpers (extras D17)', () => {
  it('routes each file to its view', () => {
    const by = (i: number) => routeOf(FILES[i]!);
    expect(by(1)).toBe('image');
    expect(by(4)).toBe('video');
    expect(by(5)).toBe('text');
    expect(by(6)).toBe('text');
    expect(by(7)).toBe('unplayable');
    expect(by(8)).toBe('unplayable');
    expect(by(9)).toBe('unplayable');
    expect(by(11)).toBe('unplayable');
    expect(routeOf(file(1, 'Guides/a.pdf', GUIDES, 'document'))).toBe('pdf');
    expect(
      routeOf(file(1, 'Guides/a.epub', GUIDES, 'document', 'external')),
    ).toBe('unplayable');
  });

  it('queues the same category in list order, leaving the player its audio', () => {
    const q = (i: number) => viewerQueue(view(), FILES[i]!).map((f) => f.index);
    expect(q(2)).toEqual([1, 2, 3]);
    expect(q(4)).toEqual([4, 9]);
    expect(q(11)).toEqual([11]);
  });

  it('zooms by stops and from the fitted scale', () => {
    expect(zoomStep(null, 1, 0.4)).toBe(0.5);
    expect(zoomStep(null, -1, 0.4)).toBe(0.25);
    expect(zoomStep(1, 1, 0.4)).toBe(1.5);
    expect(zoomStep(1, -1, 0.4)).toBe(0.75);
    expect(zoomStep(8, 1, 1)).toBe(8);
    expect(zoomStep(0.1, -1, 1)).toBe(0.1);
  });
});

describe('MediaViewer (extras D17, D23, §8.1)', () => {
  beforeEach(() => {
    reduceMotion(false);
    usePlayerStore.getState().stop();
  });
  afterEach(() => {
    act(() => useViewerStore.getState().close());
    setBackend(null);
    usePlayerStore.getState().stop();
    vi.unstubAllGlobals();
  });

  it('shows nothing while closed', () => {
    fakeBackend();
    show();
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('heads the file with its name, category and place in mono', async () => {
    fakeBackend();
    show();
    openAt('Artwork/Box Back.png');
    const title = await screen.findByRole('heading', { name: 'Box Back.png' });
    expect(title).toHaveClass('font-display');
    expect(dialog()).toHaveAccessibleName('Box Back.png');
    expect(dialog()).toHaveTextContent('Arte');
    const place = screen.getByText('2', { selector: '.mono bdi' });
    expect(place.closest('p')).toHaveTextContent('2 de 3');
  });

  it('shows an image on a checkerboard, never autoplaying anything', async () => {
    const b = fakeBackend();
    show();
    openAt('Artwork/Box Front.png');
    const img = await screen.findByRole('img', { name: 'Box Front.png' });
    expect(img).toHaveAttribute('src', 'blob:media-4-1');
    expect(img).toHaveClass('media-viewer__checker');
    expect(b.extrasMedia).toHaveBeenCalledWith(GAME.generation, 7, 4, 1);
  });

  it('←/→ go to the previous and next file of the category, and stop at the ends', async () => {
    fakeBackend();
    show();
    openAt('Artwork/Box Front.png');
    await screen.findByRole('img', { name: 'Box Front.png' });
    fireEvent.keyDown(dialog(), { key: 'ArrowLeft' });
    expect(useViewerStore.getState().open?.path).toBe('Artwork/Box Front.png');
    fireEvent.keyDown(dialog(), { key: 'ArrowRight' });
    await screen.findByRole('img', { name: 'Box Back.png' });
    fireEvent.click(screen.getByRole('button', { name: 'Archivo siguiente' }));
    await screen.findByRole('img', { name: 'Poster.jpg' });
    expect(
      screen.getByRole('button', { name: 'Archivo siguiente' }),
    ).toBeDisabled();
    fireEvent.keyDown(dialog(), { key: 'ArrowRight' });
    expect(useViewerStore.getState().open?.path).toBe('Artwork/Poster.jpg');
  });

  it('Esc closes and gives the focus back to the row', async () => {
    fakeBackend();
    show();
    const row = screen.getByRole('button', { name: 'Box Front.png' });
    row.focus();
    openAt('Artwork/Box Front.png');
    await screen.findByRole('img', { name: 'Box Front.png' });
    fireEvent.keyDown(dialog(), { key: 'ArrowRight' });
    await screen.findByRole('img', { name: 'Box Back.png' });
    fireEvent.keyDown(dialog(), { key: 'Escape' });
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(useViewerStore.getState().open).toBeNull();
    // The row of the file last shown.
    await waitFor(() =>
      expect(
        screen.getByRole('button', { name: 'Box Back.png' }),
      ).toHaveFocus(),
    );
  });

  it('+, - and 0 zoom the image and leave the cover size alone', async () => {
    fakeBackend();
    const spy = vi.fn();
    show({ spy });
    openAt('Artwork/Box Front.png');
    const img = await screen.findByRole('img', { name: 'Box Front.png' });
    Object.defineProperty(img, 'naturalWidth', { value: 400 });
    Object.defineProperty(img, 'naturalHeight', { value: 300 });
    fireEvent.load(img);
    fireEvent.keyDown(img, { key: '+' });
    expect(screen.getByText('150', { selector: 'bdi' })).toBeInTheDocument();
    fireEvent.keyDown(img, { key: '=', metaKey: true });
    expect(screen.getByText('200', { selector: 'bdi' })).toBeInTheDocument();
    expect(img.style.width).toBe('800px');
    fireEvent.keyDown(img, { key: '-' });
    expect(screen.getByText('150', { selector: 'bdi' })).toBeInTheDocument();
    fireEvent.keyDown(img, { key: '0', ctrlKey: true });
    expect(img.style.width).toBe('');
    fireEvent.click(screen.getByRole('button', { name: 'Ampliar' }));
    fireEvent.click(screen.getByRole('button', { name: 'Ajustar' }));
    expect(img.style.width).toBe('');
    expect(spy).not.toHaveBeenCalled();
    expect(screen.getByTestId('cover')).toHaveTextContent('m');
  });

  it('a zoomed image takes the focus and its arrows pan, not step', async () => {
    fakeBackend();
    show();
    openAt('Artwork/Box Front.png');
    const img = await screen.findByRole('img', { name: 'Box Front.png' });
    Object.defineProperty(img, 'naturalWidth', { value: 400 });
    Object.defineProperty(img, 'naturalHeight', { value: 300 });
    fireEvent.load(img);
    const stage = img.closest<HTMLElement>('.media-viewer__stage')!;
    // Fitted: nothing to pan, so the stage is not a tab stop.
    expect(stage).not.toHaveAttribute('tabindex');
    fireEvent.keyDown(dialog(), { key: '+' });
    expect(stage).toHaveAttribute('tabindex', '0');
    expect(stage).toHaveAccessibleName('Box Front.png');
    Object.defineProperty(stage, 'scrollWidth', { value: 1200 });
    Object.defineProperty(stage, 'clientWidth', { value: 600 });
    stage.focus();
    const right = fireEvent.keyDown(stage, { key: 'ArrowRight' });
    // Left to the browser, which scrolls the stage.
    expect(right).toBe(true);
    expect(useViewerStore.getState().open?.path).toBe('Artwork/Box Front.png');
    // From the dialog itself the arrows still step through the category.
    fireEvent.keyDown(dialog(), { key: 'ArrowRight' });
    expect(useViewerStore.getState().open?.path).toBe('Artwork/Box Back.png');
  });

  it('a wide .nfo scrolls sideways with the arrows from its text', async () => {
    fakeBackend();
    show();
    openAt('Readme.nfo');
    const pre = await screen.findByText(/TIDEWALKER/, { selector: 'pre' });
    const stage = pre.closest<HTMLElement>('.media-viewer__stage')!;
    Object.defineProperty(stage, 'scrollWidth', { value: 900 });
    Object.defineProperty(stage, 'clientWidth', { value: 500 });
    expect(fireEvent.keyDown(pre, { key: 'ArrowLeft' })).toBe(true);
    expect(useViewerStore.getState().open?.path).toBe('Readme.nfo');
  });

  it('plays video only with its own controls and pauses the mini player', async () => {
    fakeBackend();
    show();
    act(() => usePlayerStore.getState().playFrom(trackListing(), 0, GAME));
    expect(usePlayerStore.getState().playing).toBe(true);
    openAt('Videos/Trailer.webm');
    await waitFor(() => expect(dialog().querySelector('video')).not.toBeNull());
    const video = dialog().querySelector('video')!;
    expect(video).toHaveAttribute('controls');
    expect(video).not.toHaveAttribute('autoplay');
    expect(video.autoplay).toBe(false);
    expect(video).toHaveAttribute('src', 'blob:media-4-4');
    expect(usePlayerStore.getState().playing).toBe(false);
    // Fitted to the stage (globals.css), not shown at its own small size.
    expect(video.closest('.media-viewer__stage')).toHaveClass(
      'media-viewer__stage--video',
    );
  });

  it('a media error falls back to the unplayable view', async () => {
    fakeBackend();
    show();
    openAt('Videos/Trailer.webm');
    await waitFor(() => expect(dialog().querySelector('video')).not.toBeNull());
    fireEvent.error(dialog().querySelector('video')!);
    expect(
      await screen.findByText('Coverly no puede mostrar este archivo aquí.'),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Abrir con otra app' }),
    ).toBeInTheDocument();
  });

  it('shows text as text: an .nfo with a script tag reads literally', async () => {
    const b = fakeBackend();
    show();
    openAt('Readme.nfo');
    const pre = await screen.findByText(/TIDEWALKER/, { selector: 'pre' });
    expect(pre).toHaveTextContent("<script>alert('nfo')</script>");
    expect(dialog().querySelector('script')).toBeNull();
    expect(pre).toHaveClass('mono');
    expect(b.extrasReadText).toHaveBeenCalledWith(GAME.generation, 7, 4, 6);
  });

  it('says when a long text shows only its start', async () => {
    fakeBackend();
    show();
    openAt('Guides/Notes.txt');
    await screen.findByText(/Notas del juego/, { selector: 'pre' });
    expect(
      screen.getByText('El archivo es largo: solo se muestra el principio.'),
    ).toBeInTheDocument();
  });

  it('an unplayable format offers «Abrir con otra app» only when allowed', async () => {
    const b = fakeBackend();
    show();
    openAt('Themes/Tidewalker.p3t');
    await screen.findByText('Coverly no puede mostrar este archivo aquí.');
    fireEvent.click(screen.getByRole('button', { name: 'Abrir con otra app' }));
    await waitFor(() =>
      expect(b.extrasOpenExternal).toHaveBeenCalledWith(
        GAME.generation,
        7,
        4,
        7,
      ),
    );
    fireEvent.keyDown(dialog(), { key: 'ArrowRight' });
    await waitFor(() =>
      expect(useViewerStore.getState().open?.path).toBe('Themes/setup.exe'),
    );
    expect(
      screen.queryByRole('button', { name: 'Abrir con otra app' }),
    ).toBeNull();
    const reveal = screen.getByRole('button', { name: /^Mostrar en / });
    fireEvent.click(reveal);
    await waitFor(() =>
      expect(b.extrasReveal).toHaveBeenCalledWith(GAME.generation, 7, 4, 8),
    );
    expect(b.extrasMedia).not.toHaveBeenCalled();
  });

  it('a file over 200 MB is never fetched: unplayable', async () => {
    const b = fakeBackend();
    show();
    openAt('Videos/Huge.mov');
    await screen.findByText('Coverly no puede mostrar este archivo aquí.');
    expect(b.extrasMedia).not.toHaveBeenCalled();
  });

  it('an image over 100 megapixels falls back to unplayable', async () => {
    fakeBackend();
    show();
    openAt('Artwork/Poster.jpg');
    const img = await screen.findByRole('img', { name: 'Poster.jpg' });
    Object.defineProperty(img, 'naturalWidth', { value: 20_000 });
    Object.defineProperty(img, 'naturalHeight', {
      value: MAX_PIXELS / 20_000 + 1,
    });
    fireEvent.load(img);
    expect(
      await screen.findByText('Coverly no puede mostrar este archivo aquí.'),
    ).toBeInTheDocument();
  });

  it('a refused grant falls back to unplayable', async () => {
    fakeBackend({
      extrasMedia: vi.fn(async () => {
        throw new Error('refused');
      }),
    });
    show();
    openAt('Artwork/Box Front.png');
    expect(
      await screen.findByText('Coverly no puede mostrar este archivo aquí.'),
    ).toBeInTheDocument();
  });

  it('fades in, and not at all under reduced motion', async () => {
    fakeBackend();
    const { unmount } = show();
    openAt('Artwork/Box Front.png');
    await screen.findByRole('img', { name: 'Box Front.png' });
    expect(dialog()).toHaveClass('media-viewer--fade');
    unmount();
    act(() => useViewerStore.getState().close());
    reduceMotion(true);
    show();
    openAt('Artwork/Box Front.png');
    await screen.findByRole('img', { name: 'Box Front.png' });
    expect(dialog()).not.toHaveClass('media-viewer--fade');
  });

  it('closes when its file is gone from the listing, and on unmount', async () => {
    fakeBackend();
    const { unmount } = show();
    openAt('Artwork/Gone.png');
    await waitFor(() => expect(useViewerStore.getState().open).toBeNull());
    openAt('Artwork/Box Front.png');
    await screen.findByRole('img', { name: 'Box Front.png' });
    unmount();
    expect(useViewerStore.getState().open).toBeNull();
  });
});
