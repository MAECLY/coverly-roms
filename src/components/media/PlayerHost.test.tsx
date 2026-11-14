import { act, fireEvent, render, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '@/i18n/I18nProvider';
import { setBackend } from '@/ipc/backend';
import type { LibraryView } from '@/ipc/types';
import { useLibraryStore } from '@/stores/libraryStore';
import { useNavStore } from '@/stores/navStore';
import { usePlayerStore } from '@/stores/playerStore';

import { PlayerHost } from './PlayerHost';
import { FILES, GAME, listing, mediaBackend } from './testTracks';

const store = () => usePlayerStore.getState();
let play: ReturnType<typeof vi.spyOn>;
let pause: ReturnType<typeof vi.spyOn>;
let media: ReturnType<typeof mediaBackend>;

function host() {
  const r = render(
    <I18nProvider locale="es">
      <PlayerHost />
    </I18nProvider>,
  );
  const audio = () => r.container.querySelector('audio')!;
  return { ...r, audio };
}

/** The element's clock, as the fake media element reports it. */
function tick(el: HTMLAudioElement, currentTime: number, duration: number) {
  Object.defineProperty(el, 'currentTime', {
    configurable: true,
    writable: true,
    value: currentTime,
  });
  Object.defineProperty(el, 'duration', {
    configurable: true,
    value: duration,
  });
  fireEvent.timeUpdate(el);
}

describe('PlayerHost (extras spec D19)', () => {
  beforeEach(() => {
    play = vi
      .spyOn(HTMLMediaElement.prototype, 'play')
      .mockImplementation(() => Promise.resolve());
    pause = vi
      .spyOn(HTMLMediaElement.prototype, 'pause')
      .mockImplementation(() => undefined);
    media = mediaBackend();
    setBackend(media.backend);
    store().stop();
  });
  afterEach(() => {
    store().stop();
    useLibraryStore.setState({ view: null });
    useNavStore.setState({ screen: 'main' });
    setBackend(null);
    vi.restoreAllMocks();
  });

  it('never plays on mount: one silent element, no autoplay', () => {
    const { audio } = host();
    expect(audio()).not.toHaveAttribute('autoplay');
    expect(audio()).not.toHaveAttribute('src');
    expect(play).not.toHaveBeenCalled();
  });

  it('plays the track the user started, from its granted URL', async () => {
    const { audio } = host();
    act(() => store().playFrom(listing(), 1, GAME));
    await waitFor(() =>
      expect(audio()).toHaveAttribute('src', 'blob:track-1-1'),
    );
    expect(media.extrasMedia).toHaveBeenCalledWith(3, 7, 1, 1);
    await waitFor(() => expect(play).toHaveBeenCalled());
  });

  it('pauses, seeks, reports time and goes on to the next track', async () => {
    const { audio } = host();
    act(() => store().playFrom(listing(), 0, GAME));
    await waitFor(() =>
      expect(audio()).toHaveAttribute('src', 'blob:track-1-0'),
    );
    act(() => tick(audio(), 12, 200));
    expect(store().position).toBe(12);
    expect(store().duration).toBe(200);
    act(() => store().pause());
    expect(pause).toHaveBeenCalled();
    act(() => store().seek(42));
    expect(audio().currentTime).toBe(42);
    act(() => {
      store().play();
      fireEvent.ended(audio());
    });
    expect(store().queue[store().at]!.name).toBe('02 Boss.wav');
    await waitFor(() =>
      expect(audio()).toHaveAttribute('src', 'blob:track-1-1'),
    );
  });

  it('pauses when a video opens', async () => {
    const { audio } = host();
    act(() => store().playFrom(listing(), 0, GAME));
    await waitFor(() => expect(audio()).toHaveAttribute('src'));
    pause.mockClear();
    act(() => store().pauseForVideo());
    expect(pause).toHaveBeenCalled();
    expect(store().queue).toHaveLength(3);
  });

  it('skips a trashed track to the next one', async () => {
    const { audio } = host();
    act(() => store().playFrom(listing(), 1, GAME));
    await waitFor(() =>
      expect(audio()).toHaveAttribute('src', 'blob:track-1-1'),
    );
    const left = FILES.filter((f) => f.index !== 1).map((f, i) => ({
      ...f,
      index: i,
    }));
    act(() => store().syncListing(listing(left, 2)));
    await waitFor(() =>
      expect(audio()).toHaveAttribute('src', 'blob:track-2-2'),
    );
  });

  it('a file it cannot load moves on instead of stalling', async () => {
    const { audio } = host();
    act(() => store().playFrom(listing(), 0, GAME));
    await waitFor(() => expect(audio()).toHaveAttribute('src'));
    act(() => {
      fireEvent.error(audio());
    });
    expect(store().queue[store().at]!.name).toBe('02 Boss.wav');
  });

  it('stops when its library closes', async () => {
    useLibraryStore.setState({
      view: { generation: 3 } as unknown as LibraryView,
    });
    const { audio } = host();
    act(() => store().playFrom(listing(), 0, GAME));
    await waitFor(() => expect(audio()).toHaveAttribute('src'));
    act(() => useLibraryStore.setState({ view: null }));
    expect(store().queue).toHaveLength(0);
    expect(audio()).not.toHaveAttribute('src');
  });

  it('keeps playing when another library view of the same one arrives', async () => {
    const view = { generation: 3 } as unknown as LibraryView;
    useLibraryStore.setState({ view });
    host();
    act(() => store().playFrom(listing(), 0, GAME));
    act(() => useLibraryStore.setState({ view: { ...view } }));
    expect(store().queue).toHaveLength(3);
  });

  it('«Reproducir» on the track it holds starts it again from the top', async () => {
    const { audio } = host();
    act(() => store().playFrom(listing(), 0, GAME));
    await waitFor(() =>
      expect(audio()).toHaveAttribute('src', 'blob:track-1-0'),
    );
    act(() => tick(audio(), 95, 200));
    act(() => store().pause());
    act(() => store().playFrom(listing(), 0, GAME));
    expect(audio().currentTime).toBe(0);
    expect(store().playing).toBe(true);
  });

  it('pauses on a screen without its controls, keeping the queue', async () => {
    const { audio } = host();
    act(() => store().playFrom(listing(), 0, GAME));
    await waitFor(() => expect(audio()).toHaveAttribute('src'));
    act(() => useNavStore.setState({ screen: 'settings' }));
    expect(store().playing).toBe(false);
    expect(store().queue).toHaveLength(3);
    // Back on the library it waits for the user's «Reproducir».
    act(() => useNavStore.setState({ screen: 'main' }));
    expect(store().playing).toBe(false);
  });

  it('announces track changes in a polite live region', async () => {
    const { container } = host();
    const live = container.querySelector('[aria-live="polite"]')!;
    expect(live).toHaveTextContent('');
    act(() => store().playFrom(listing(), 0, GAME));
    expect(live).toHaveTextContent('01 Opening.flac, Tidewalker');
    act(() => store().next());
    expect(live).toHaveTextContent('02 Boss.wav, Tidewalker');
  });
});
