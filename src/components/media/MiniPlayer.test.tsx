import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import type { LibraryView } from '@/ipc/types';
import { useLibraryStore } from '@/stores/libraryStore';
import { usePlayerStore } from '@/stores/playerStore';
import { useSelectionStore } from '@/stores/selectionStore';
import { useViewStore } from '@/stores/viewStore';

import { formatTime } from '@/lib/mediaTime';

import { MiniPlayer } from './MiniPlayer';
import { GAME, listing } from './testTracks';

const store = () => usePlayerStore.getState();

function show(variant: 'panel' | 'strip' = 'panel') {
  return render(
    <I18nProvider locale="es">
      <MiniPlayer variant={variant} />
    </I18nProvider>,
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

describe('MiniPlayer (extras spec D19, §8.3, §8.4)', () => {
  beforeEach(() => {
    store().stop();
    reduceMotion(false);
  });
  afterEach(() => {
    store().stop();
    useLibraryStore.setState({ view: null });
    useSelectionStore.getState().select(null);
    useViewStore.getState().closeDrawer();
    vi.unstubAllGlobals();
  });

  it('shows nothing while nothing is queued', () => {
    const { container } = show();
    expect(container).toBeEmptyDOMElement();
  });

  it('plays, pauses, goes next and back, and stops', () => {
    show();
    act(() => store().playFrom(listing(), 0, GAME));
    fireEvent.click(screen.getByRole('button', { name: 'Pausar' }));
    expect(store().playing).toBe(false);
    fireEvent.click(screen.getByRole('button', { name: 'Reproducir' }));
    expect(store().playing).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: 'Siguiente pista' }));
    expect(store().at).toBe(1);
    fireEvent.click(screen.getByRole('button', { name: 'Pista anterior' }));
    expect(store().at).toBe(0);
    fireEvent.click(screen.getByRole('button', { name: 'Detener' }));
    expect(store().queue).toHaveLength(0);
  });

  it('names the group by the track and shows the time in mono', () => {
    show();
    act(() => {
      store().playFrom(listing(), 1, GAME);
      store().progress(72, 220);
    });
    const group = screen.getByRole('group', { name: /02 Boss\.wav/ });
    expect(group).toHaveTextContent('Tidewalker');
    const time = group.querySelector('.mini-player__time')!;
    expect(time).toHaveClass('mono');
    expect(time).toHaveTextContent('1:12 / 3:40');
  });

  it('seeks with the slider: arrows ±5 s, PageUp/PageDown ±30 s', () => {
    show();
    act(() => {
      store().playFrom(listing(), 0, GAME);
      store().progress(60, 300);
    });
    const thumb = screen.getByRole('slider', { name: 'Posición' });
    expect(thumb).toHaveAttribute('aria-valuetext', '1:00 de 5:00');
    fireEvent.keyDown(thumb, { key: 'ArrowRight' });
    expect(store().position).toBe(65);
    fireEvent.keyDown(thumb, { key: 'ArrowLeft' });
    expect(store().position).toBe(60);
    fireEvent.keyDown(thumb, { key: 'PageUp' });
    expect(store().position).toBe(90);
    fireEvent.keyDown(thumb, { key: 'PageDown' });
    expect(store().position).toBe(60);
    fireEvent.keyDown(thumb, { key: 'PageDown' });
    fireEvent.keyDown(thumb, { key: 'PageDown' });
    expect(store().position).toBe(0);
  });

  it('the game name selects the game and opens its extras', () => {
    useLibraryStore.setState({
      view: { generation: 3 } as unknown as LibraryView,
    });
    show();
    act(() => store().playFrom(listing(), 0, GAME));
    // Named for where it leads, the game's visible name inside the name.
    fireEvent.click(
      screen.getByRole('button', { name: 'Ver los extras de Tidewalker' }),
    );
    expect(useSelectionStore.getState().selectedId).toBe(7);
    expect(useViewStore.getState().drawerOpen).toBe(true);
    expect(useViewStore.getState().panelSection).toBe('extras');
  });

  it('the strip keeps only the track, play/pause and stop', () => {
    show('strip');
    act(() => store().playFrom(listing(), 0, GAME));
    const group = screen.getByRole('group', { name: /01 Opening\.flac/ });
    expect(group).toHaveClass('mini-player--strip');
    expect(screen.getByRole('button', { name: 'Pausar' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Detener' })).toBeInTheDocument();
    expect(screen.queryByRole('slider')).toBeNull();
    expect(
      screen.queryByRole('button', { name: 'Siguiente pista' }),
    ).toBeNull();
  });

  it('turns transitions off under reduced motion', () => {
    reduceMotion(true);
    show();
    act(() => store().playFrom(listing(), 0, GAME));
    expect(screen.getByRole('group')).toHaveAttribute(
      'data-reduced-motion',
      'true',
    );
  });

  it('formats times as m:ss and h:mm:ss', () => {
    expect(formatTime(0)).toBe('0:00');
    expect(formatTime(72.9)).toBe('1:12');
    expect(formatTime(3725)).toBe('1:02:05');
    expect(formatTime(Number.NaN)).toBe('0:00');
  });
});
