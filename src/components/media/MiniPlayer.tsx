import { Pause, Play, SkipBack, SkipForward, Square } from 'lucide-react';
import { useId, type KeyboardEvent } from 'react';

import { Slider } from '@/components/ui/slider';
import { useT } from '@/i18n/useT';
import { formatTime } from '@/lib/mediaTime';
import { useReducedMotion } from '@/lib/useReducedMotion';
import { useLibraryStore } from '@/stores/libraryStore';
import { currentTrack, usePlayerStore } from '@/stores/playerStore';
import { useSelectionStore } from '@/stores/selectionStore';
import { useViewStore } from '@/stores/viewStore';

/** Arrow keys on the seek slider move this many seconds… */
const STEP = 5;
/** …and PageUp/PageDown this many. */
const PAGE = 30;

/**
 * The mini player's controls (extras spec D19, §8.3): a 48 px band pinned
 * to the bottom of the game panel (`panel`: previous, play/pause, next,
 * the track and its game, «Detener», the seek slider and the time in
 * mono), or, with the panel closed, a 32 px strip above the status bar
 * (`strip`: play/pause, the track, «Detener»). The `<audio>` itself is in
 * `PlayerHost`; this only drives `playerStore`. Nothing renders while
 * nothing is queued.
 */
export function MiniPlayer({ variant }: { variant: 'panel' | 'strip' }) {
  const t = useT();
  const id = useId();
  const reduced = useReducedMotion();
  const track = usePlayerStore(currentTrack);
  const playing = usePlayerStore((s) => s.playing);
  const position = usePlayerStore((s) => s.position);
  const duration = usePlayerStore((s) => s.duration);
  const atLast = usePlayerStore((s) => s.at + 1 >= s.queue.length);
  const generation = useLibraryStore((s) => s.view?.generation);
  if (!track) return null;
  const p = usePlayerStore.getState();
  const strip = variant === 'strip';
  const ownLibrary = track.generation === generation;

  function selectGame() {
    if (!track || !ownLibrary) return;
    useSelectionStore.getState().select(track.entryId);
    useViewStore.getState().openDrawer('extras');
  }

  function seekKeys(e: KeyboardEvent) {
    const by: Record<string, number> = {
      ArrowRight: STEP,
      ArrowUp: STEP,
      ArrowLeft: -STEP,
      ArrowDown: -STEP,
      PageUp: PAGE,
      PageDown: -PAGE,
    };
    const delta = by[e.key];
    if (delta === undefined) return;
    // Radix would snap to its step and page by ten steps: ours instead.
    e.preventDefault();
    const s = usePlayerStore.getState();
    s.seek(s.position + delta);
  }

  const toggle = (
    <button
      type="button"
      className="mini-player__btn mini-player__btn--main"
      aria-label={playing ? t('player.pause') : t('player.play')}
      onClick={() => p.toggle()}
    >
      {playing ? <Pause aria-hidden /> : <Play aria-hidden />}
    </button>
  );
  const stop = (
    <button
      type="button"
      className="mini-player__btn"
      aria-label={t('player.stop')}
      onClick={() => p.stop()}
    >
      <Square aria-hidden />
    </button>
  );
  const line = (
    <span className="mini-player__line">
      <span id={id} className="mini-player__track" title={track.name}>
        {track.name}
      </span>
      <span aria-hidden="true"> · </span>
      <button
        type="button"
        className="mini-player__game"
        aria-label={t('player.showGame', { game: track.game })}
        disabled={!ownLibrary}
        onClick={selectGame}
      >
        {track.game}
      </button>
    </span>
  );

  return (
    <div
      role="group"
      aria-labelledby={id}
      className={`mini-player mini-player--${variant}`}
      data-reduced-motion={reduced || undefined}
      data-playing={playing || undefined}
    >
      {strip ? (
        <>
          {toggle}
          {line}
          {stop}
        </>
      ) : (
        <>
          <div className="mini-player__row">
            <button
              type="button"
              className="mini-player__btn"
              aria-label={t('player.previous')}
              onClick={() => p.previous()}
            >
              <SkipBack aria-hidden />
            </button>
            {toggle}
            <button
              type="button"
              className="mini-player__btn"
              aria-label={t('player.next')}
              disabled={atLast}
              onClick={() => p.next()}
            >
              <SkipForward aria-hidden />
            </button>
            {line}
            {stop}
          </div>
          <div className="mini-player__row">
            <Slider
              className="mini-player__seek"
              min={0}
              max={duration > 0 ? duration : 1}
              step={1}
              value={[Math.min(position, duration > 0 ? duration : 0)]}
              disabled={duration <= 0}
              thumbLabel={t('player.seek')}
              thumbValueText={t('viewer.of', {
                i: formatTime(position),
                n: formatTime(duration),
              })}
              onKeyDown={seekKeys}
              onValueChange={([v]) => {
                if (v !== undefined) p.seek(v);
              }}
            />
            <span className="mini-player__time mono">
              {formatTime(position)} / {formatTime(duration)}
            </span>
          </div>
        </>
      )}
    </div>
  );
}
