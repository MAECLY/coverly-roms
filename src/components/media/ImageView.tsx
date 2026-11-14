import { useEffect, useRef, useState } from 'react';

import { MAX_PIXELS } from '@/lib/viewer';

/**
 * One image of the viewer (extras §8.1): centred on a quiet checkerboard,
 * because transparency is information for artwork. `zoom` is a scale of
 * the image's own pixels, or `null` to fit the stage; the stage scrolls
 * when the image is larger. Reports the scale that fits, and gives up
 * (`onUnplayable`) on a load error or an image over 100 megapixels.
 */
export function ImageView({
  url,
  name,
  zoom,
  onFit,
  onUnplayable,
}: {
  url: string;
  name: string;
  zoom: number | null;
  onFit(scale: number): void;
  onUnplayable(): void;
}) {
  const stage = useRef<HTMLDivElement>(null);
  const [n, setNatural] = useState<{ w: number; h: number } | null>(null);
  const latest = useRef({ n, onFit });
  useEffect(() => {
    latest.current = { n, onFit };
  });

  function measure(sizeOf = latest.current.n) {
    const box = stage.current;
    if (!sizeOf || !box) return;
    const w = box.clientWidth;
    const h = box.clientHeight;
    // No layout (a test, a hidden stage): the image's own size fits.
    latest.current.onFit(
      w > 0 && h > 0 ? Math.min(1, w / sizeOf.w, h / sizeOf.h) : 1,
    );
  }

  useEffect(() => {
    const box = stage.current;
    if (!box || typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(() => measure());
    ro.observe(box);
    return () => ro.disconnect();
  }, []);

  const size =
    zoom !== null && n
      ? { width: `${Math.round(n.w * zoom)}px`, height: 'auto' }
      : undefined;

  return (
    // Zoomed, the stage scrolls: a tab stop so the keyboard pans it.
    <div
      ref={stage}
      className="media-viewer__stage"
      data-zoomed={zoom !== null || undefined}
      role={zoom !== null ? 'group' : undefined}
      aria-label={zoom !== null ? name : undefined}
      tabIndex={zoom !== null ? 0 : undefined}
    >
      <div className="media-viewer__canvas">
        <img
          src={url}
          alt={name}
          draggable={false}
          className="media-viewer__checker media-viewer__image"
          style={size}
          onLoad={(e) => {
            const el = e.currentTarget;
            if (el.naturalWidth * el.naturalHeight > MAX_PIXELS) {
              onUnplayable();
              return;
            }
            const size = { w: el.naturalWidth, h: el.naturalHeight };
            setNatural(size);
            measure(size);
          }}
          onError={onUnplayable}
        />
      </div>
    </div>
  );
}
