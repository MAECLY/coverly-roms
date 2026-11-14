import { useEffect, useRef, useState } from 'react';

import { getBackend } from '@/ipc/backend';
import { useLibraryStore } from '@/stores/libraryStore';
import { useNavStore } from '@/stores/navStore';
import { currentTrack, usePlayerStore } from '@/stores/playerStore';

/**
 * The one `<audio>` of the mini player (extras spec D19). It lives beside
 * the `Toaster`, outside the game panel that unmounts when no game is
 * selected, so a track keeps playing while the user browses other games.
 * It follows `playerStore` (track, play/pause, seeks), reports time and
 * the end of a track back, asks for each track's URL by its listing
 * handle (never a path), stops when the track's library closes, pauses
 * on screens without its controls and never plays before the user's
 * «Reproducir».
 */
export function PlayerHost() {
  const ref = useRef<HTMLAudioElement>(null);
  const track = usePlayerStore(currentTrack);
  const playing = usePlayerStore((s) => s.playing);
  const seekNonce = usePlayerStore((s) => s.seekNonce);
  // The file the element holds: the same path keeps its URL when a new
  // listing only moves its handle.
  const key = track
    ? `${track.generation}:${track.entryId}:${track.path}`
    : null;
  const [src, setSrc] = useState<{ key: string; url: string } | null>(null);
  const url = src && src.key === key ? src.url : null;

  useEffect(() => {
    if (!key) return;
    const t = usePlayerStore.getState();
    const want = currentTrack(t);
    if (!want) return;
    let live = true;
    getBackend()
      .then((b) =>
        b.extrasMedia(want.generation, want.entryId, want.rev, want.index),
      )
      .then(
        (m) => {
          if (live) setSrc({ key, url: m.url });
        },
        () => {
          // Not granted (gone, refused): the next track, as a load error.
          if (live) usePlayerStore.getState().next();
        },
      );
    return () => {
      live = false;
    };
  }, [key]);

  // Play or pause the element as the store says, once it has its file.
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (!url) {
      el.pause();
      return;
    }
    if (playing) {
      el.play()?.catch(() => usePlayerStore.getState().pause());
    } else el.pause();
  }, [url, playing]);

  const lastSeek = useRef(seekNonce);
  useEffect(() => {
    if (seekNonce === lastSeek.current) return;
    lastSeek.current = seekNonce;
    const el = ref.current;
    if (el && url) el.currentTime = usePlayerStore.getState().position;
  }, [seekNonce, url]);

  // Closing the library (or opening another) stops its track.
  useEffect(
    () =>
      useLibraryStore.subscribe((s, prev) => {
        const was = prev.view?.generation;
        if (was !== undefined && s.view?.generation !== was)
          usePlayerStore.getState().stopOnLibraryClose(was);
      }),
    [],
  );

  // Settings, Organizar and Duplicados show no player controls: the
  // music pauses there (it must never play without a way to stop it, and
  // Organizar may move the file) and waits for «Reproducir» back home.
  useEffect(
    () =>
      useNavStore.subscribe((s, prev) => {
        if (s.screen !== prev.screen && s.screen !== 'main')
          usePlayerStore.getState().pause();
      }),
    [],
  );

  const report = (el: HTMLAudioElement) =>
    usePlayerStore
      .getState()
      .progress(el.currentTime, Number.isFinite(el.duration) ? el.duration : 0);

  return (
    <>
      {/* Music from the user's own extras: no captions track to offer. */}
      {/* eslint-disable-next-line jsx-a11y/media-has-caption */}
      <audio
        ref={ref}
        preload="metadata"
        hidden
        {...(url ? { src: url } : {})}
        onTimeUpdate={(e) => report(e.currentTarget)}
        onLoadedMetadata={(e) => report(e.currentTarget)}
        onDurationChange={(e) => report(e.currentTarget)}
        onEnded={() => usePlayerStore.getState().ended()}
        onError={() => {
          if (url) usePlayerStore.getState().next();
        }}
      />
      <p className="sr-only" aria-live="polite">
        {track ? `${track.name}, ${track.game}` : ''}
      </p>
    </>
  );
}
