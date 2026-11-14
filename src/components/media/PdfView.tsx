import { ExternalLink } from 'lucide-react';
import type {
  PDFDocumentLoadingTask,
  PDFDocumentProxy,
  RenderTask,
  TextLayer,
} from 'pdfjs-dist/legacy/build/pdf.mjs';
import {
  useCallback,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
  type CSSProperties,
  type Ref,
} from 'react';

import { Button } from '@/components/ui/button';
import { Spinner } from '@/components/ui/spinner';
import { useT } from '@/i18n/useT';
import { openPdf, type PdfjsModule } from '@/lib/pdfjs';
import { useReducedMotion } from '@/lib/useReducedMotion';

/** At most this many pages hold a rendered canvas at once (D18). */
export const MAX_CANVASES = 3;

/** PDF points → CSS pixels at 100 %. */
const CSS_UNITS = 96 / 72;

/** The stage's side padding (1.5rem each side), out of the fitted width. */
const SIDE_PADDING = 48;

export type PdfStep = 'next' | 'previous' | 'first' | 'last';

/** What the viewer drives from its keys (PageUp/PageDown/Home/End). */
export interface PdfHandle {
  step(to: PdfStep): void;
}

interface Opened {
  pdfjs: PdfjsModule;
  doc: PDFDocumentProxy;
  /** Page 1's size in points: every page's box until its own is known. */
  base: { w: number; h: number };
}

/**
 * A PDF in the Extras viewer (extras D18, §8.1): pages stacked vertically
 * on the stage, each a canvas with pdf.js's text layer over it (selectable
 * and read by screen readers). Only the pages near the viewport render,
 * and at most three canvases exist at a time; the rest are boxes of the
 * right size. pdf.js is imported when the first PDF opens. A page that
 * cannot be drawn (an image codec that needs WebAssembly) says so and
 * offers «Abrir con otra app» when the type allows it; a document pdf.js
 * cannot open falls back to the unplayable view.
 */
export function PdfView({
  url,
  name,
  zoom,
  onFit,
  onPage,
  onUnplayable,
  canExternal,
  onOpenExternal,
  ref,
}: {
  url: string;
  name: string;
  /** A scale of the page's own size (100 % = 1), or `null` to fit. */
  zoom: number | null;
  onFit(scale: number): void;
  onPage(page: number, pages: number): void;
  onUnplayable(): void;
  canExternal: boolean;
  onOpenExternal(): void;
  ref?: Ref<PdfHandle>;
}) {
  const t = useT();
  const reduced = useReducedMotion();
  const stage = useRef<HTMLDivElement>(null);
  const boxes = useRef(new Map<number, HTMLDivElement>());
  const [opened, setOpened] = useState<Opened | null>(null);
  const [width, setWidth] = useState(0);
  const [near, setNear] = useState<ReadonlySet<number>>(() => new Set([1]));
  const [current, setCurrent] = useState(1);
  const [sizes, setSizes] = useState<
    Readonly<Record<number, { w: number; h: number }>>
  >({});
  const latest = useRef({ onFit, onPage, onUnplayable });
  useEffect(() => {
    latest.current = { onFit, onPage, onUnplayable };
  });

  // Open the document; destroy it (and its worker) when done.
  useEffect(() => {
    let live = true;
    let task: PDFDocumentLoadingTask | null = null;
    openPdf(url)
      .then(async ({ pdfjs, task: started }) => {
        task = started;
        // Closed while pdf.js loaded: the cleanup had no task to destroy.
        if (!live) {
          void started.destroy();
          return;
        }
        const doc = await started.promise;
        const first = await doc.getPage(1);
        const v = first.getViewport({ scale: 1 });
        if (live) setOpened({ pdfjs, doc, base: { w: v.width, h: v.height } });
      })
      .catch(() => {
        if (live) latest.current.onUnplayable();
      });
    return () => {
      live = false;
      void task?.destroy();
    };
  }, [url]);

  // The stage's width, for the fitted scale.
  useEffect(() => {
    const box = stage.current;
    if (!box) return;
    const measure = () => setWidth(box.clientWidth);
    measure();
    if (typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(measure);
    ro.observe(box);
    return () => ro.disconnect();
  }, [opened]);

  const pages = opened?.doc.numPages ?? 0;
  const base = opened?.base ?? null;
  // No layout (a test, a hidden stage): the page's own size fits.
  const fit =
    base && width > SIDE_PADDING
      ? (width - SIDE_PADDING) / (base.w * CSS_UNITS)
      : 1;
  const scale = (zoom ?? fit) * CSS_UNITS;

  useEffect(() => {
    if (opened) latest.current.onFit(fit);
  }, [opened, fit]);

  useEffect(() => {
    if (pages > 0) latest.current.onPage(current, pages);
  }, [current, pages]);

  // Which page boxes are near the viewport (one screen above and below).
  useEffect(() => {
    const root = stage.current;
    if (!opened || !root || typeof IntersectionObserver === 'undefined') return;
    const io = new IntersectionObserver(
      (entries) =>
        setNear((was) => {
          const next = new Set(was);
          for (const e of entries) {
            const n = Number(e.target.getAttribute('data-page'));
            if (e.isIntersecting) next.add(n);
            else next.delete(n);
          }
          return next;
        }),
      { root, rootMargin: '100% 0px' },
    );
    for (const el of boxes.current.values()) io.observe(el);
    return () => io.disconnect();
  }, [opened]);

  // A zoom keeps the current page in view.
  const scaleRef = useRef(scale);
  useEffect(() => {
    if (scaleRef.current === scale) return;
    scaleRef.current = scale;
    boxes.current.get(current)?.scrollIntoView?.({ block: 'start' });
    // Only on a new scale, not when the current page changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scale]);

  // The current page: the last one whose top is above 40 % of the stage.
  function onScroll() {
    const root = stage.current;
    if (!root || root.clientHeight === 0) return;
    const line = root.scrollTop + root.clientHeight * 0.4;
    let at = 1;
    for (const [n, el] of boxes.current) {
      if (el.offsetTop <= line && n > at) at = n;
    }
    setCurrent(at);
  }

  useImperativeHandle(
    ref,
    () => ({
      step(to) {
        if (pages === 0) return;
        const target =
          to === 'first'
            ? 1
            : to === 'last'
              ? pages
              : Math.min(
                  pages,
                  Math.max(1, current + (to === 'next' ? 1 : -1)),
                );
        setCurrent(target);
        boxes.current.get(target)?.scrollIntoView?.({
          block: 'start',
          behavior: reduced ? 'auto' : 'smooth',
        });
      },
    }),
    [pages, current, reduced],
  );

  const register = useCallback((n: number, el: HTMLDivElement | null) => {
    if (el) boxes.current.set(n, el);
    else boxes.current.delete(n);
  }, []);

  const onSize = useCallback((n: number, w: number, h: number) => {
    setSizes((s) =>
      s[n] && s[n].w === w && s[n].h === h ? s : { ...s, [n]: { w, h } },
    );
  }, []);

  // The pages that hold a canvas: the nearest to the current one.
  const live = new Set(
    [...near]
      .filter((n) => n >= 1 && n <= pages)
      .sort((a, b) => Math.abs(a - current) - Math.abs(b - current) || a - b)
      .slice(0, MAX_CANVASES),
  );

  return (
    <div
      ref={stage}
      className="media-viewer__stage media-viewer__stage--pdf"
      role="document"
      aria-label={name}
      // A scrolling region the keyboard must reach (the arrows scroll it).
      // eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex
      tabIndex={0}
      onScroll={onScroll}
    >
      {!opened || !base ? (
        <div className="media-viewer__loading">
          <Spinner size="lg" />
        </div>
      ) : (
        <div className="pdf-pages">
          {Array.from({ length: pages }, (_, i) => i + 1).map((n) => {
            const own = sizes[n] ?? base;
            return (
              <PdfPage
                key={n}
                n={n}
                pages={pages}
                opened={opened}
                scale={scale}
                width={Math.round(own.w * scale)}
                height={Math.round(own.h * scale)}
                live={live.has(n)}
                register={register}
                onSize={onSize}
                failedLabel={t('viewer.pageFailed')}
                label={t('viewer.page', { i: n, n: pages })}
                externalLabel={canExternal ? t('viewer.openExternal') : null}
                onOpenExternal={onOpenExternal}
              />
            );
          })}
        </div>
      )}
    </div>
  );
}

/**
 * One page box. While `live` it draws its canvas (sharp on HiDPI, at most
 * 2× the CSS size) and its text layer; when it leaves the window the
 * canvas goes, and its memory with it.
 */
function PdfPage({
  n,
  pages,
  opened,
  scale,
  width,
  height,
  live,
  register,
  onSize,
  label,
  failedLabel,
  externalLabel,
  onOpenExternal,
}: {
  n: number;
  pages: number;
  opened: Opened;
  scale: number;
  width: number;
  height: number;
  live: boolean;
  register(n: number, el: HTMLDivElement | null): void;
  onSize(n: number, w: number, h: number): void;
  label: string;
  failedLabel: string;
  externalLabel: string | null;
  onOpenExternal(): void;
}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const text = useRef<HTMLDivElement>(null);
  const [failed, setFailed] = useState(false);
  const box = useCallback(
    (el: HTMLDivElement | null) => register(n, el),
    [n, register],
  );

  useEffect(() => {
    if (!live || failed) return;
    const { pdfjs, doc } = opened;
    let cancelled = false;
    let render: RenderTask | null = null;
    let layer: TextLayer | null = null;
    const el = canvas.current;
    doc
      .getPage(n)
      .then((page) => {
        if (cancelled || !el) return;
        const own = page.getViewport({ scale: 1 });
        onSize(n, own.width, own.height);
        const viewport = page.getViewport({ scale });
        const ratio = Math.min(window.devicePixelRatio || 1, 2);
        el.width = Math.floor(viewport.width * ratio);
        el.height = Math.floor(viewport.height * ratio);
        render = page.render({
          canvas: el,
          viewport,
          ...(ratio !== 1 ? { transform: [ratio, 0, 0, ratio, 0, 0] } : {}),
        });
        const holder = text.current;
        if (holder) {
          holder.replaceChildren();
          layer = new pdfjs.TextLayer({
            textContentSource: page.streamTextContent(),
            container: holder,
            viewport,
          });
          // Text is a bonus: a page without it still shows.
          layer.render().catch(() => undefined);
        }
        return render.promise;
      })
      .catch((e: unknown) => {
        if (cancelled || e instanceof pdfjs.RenderingCancelledException) return;
        setFailed(true);
      });
    return () => {
      cancelled = true;
      render?.cancel();
      layer?.cancel();
      // Hand the bitmap back now; WebKit keeps it until GC otherwise.
      if (el) {
        el.width = 0;
        el.height = 0;
      }
    };
  }, [live, failed, opened, n, scale, onSize]);

  const style = {
    width: `${width}px`,
    height: `${height}px`,
    '--total-scale-factor': String(scale),
    '--scale-round-x': '1px',
    '--scale-round-y': '1px',
  } as CSSProperties;

  return (
    <div
      ref={box}
      className="pdf-page"
      data-page={n}
      data-pages={pages}
      role="group"
      aria-label={label}
      style={style}
    >
      {failed ? (
        <div className="pdf-page__failed">
          <p>{failedLabel}</p>
          {externalLabel && (
            <Button size="sm" variant="secondary" onClick={onOpenExternal}>
              <ExternalLink aria-hidden />
              {externalLabel}
            </Button>
          )}
        </div>
      ) : (
        live && (
          <>
            <canvas ref={canvas} className="pdf-page__canvas" aria-hidden />
            <div ref={text} className="textLayer" />
          </>
        )
      )}
    </div>
  );
}
